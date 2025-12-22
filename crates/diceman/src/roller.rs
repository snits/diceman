// ABOUTME: Dice rolling and expression evaluation logic.
// ABOUTME: Evaluates parsed AST nodes to produce roll results.

use crate::ast::{Compare, Condition, Expr, Modifier, Op, Roll, Sides};
use crate::error::{Error, Result};
use std::fmt;

/// Maximum number of explosions/rerolls allowed to prevent infinite loops.
const MAX_EXPLOSIONS: u32 = 100;
const MAX_REROLLS: u32 = 100;

/// Trait for random number generation, allowing for testing with fixed values.
pub trait Rng {
    /// Generate a random number in the range [1, max].
    fn roll(&mut self, max: u32) -> u32;
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
pub struct DieResult {
    /// The final value of this die (after any modifications).
    pub value: i64,
    /// The original rolled values (before explosions).
    pub rolls: Vec<i64>,
    /// Whether this die was dropped/discarded.
    pub dropped: bool,
}

/// Result of evaluating a dice expression.
#[derive(Debug, Clone)]
pub struct RollResult {
    /// The total value of the expression.
    pub total: i64,
    /// Individual die results (if the expression was a roll).
    pub dice: Vec<DieResult>,
    /// Formatted expression showing the roll.
    pub expression: String,
}

impl fmt::Display for RollResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.expression)
    }
}

/// Evaluate a dice expression with the default RNG.
pub fn evaluate(expr: &Expr) -> Result<RollResult> {
    evaluate_with_rng(expr, &mut FastRng::new())
}

/// Evaluate a dice expression with a custom RNG.
pub fn evaluate_with_rng(expr: &Expr, rng: &mut impl Rng) -> Result<RollResult> {
    let mut evaluator = Evaluator { rng };
    evaluator.evaluate(expr)
}

struct Evaluator<'a, R: Rng> {
    rng: &'a mut R,
}

impl<R: Rng> Evaluator<'_, R> {
    fn evaluate(&mut self, expr: &Expr) -> Result<RollResult> {
        match expr {
            Expr::Number(n) => Ok(RollResult {
                total: *n,
                dice: vec![],
                expression: n.to_string(),
            }),
            Expr::Roll(roll) => self.evaluate_roll(roll),
            Expr::BinOp { op, left, right } => {
                let left_result = self.evaluate(left)?;
                let right_result = self.evaluate(right)?;
                let total = match op {
                    Op::Add => left_result.total + right_result.total,
                    Op::Sub => left_result.total - right_result.total,
                    Op::Mul => left_result.total * right_result.total,
                    Op::Div => {
                        if right_result.total == 0 {
                            return Err(Error::DivisionByZero);
                        }
                        left_result.total / right_result.total
                    }
                };
                let expression =
                    format!("{} {} {} = {}", left_result.expression, op, right_result.expression, total);
                Ok(RollResult {
                    total,
                    dice: vec![],
                    expression,
                })
            }
            Expr::Group(inner) => {
                let result = self.evaluate(inner)?;
                Ok(RollResult {
                    total: result.total,
                    dice: result.dice,
                    expression: format!("({})", result.expression),
                })
            }
        }
    }

    fn evaluate_roll(&mut self, roll: &Roll) -> Result<RollResult> {
        // Roll the dice
        let mut dice: Vec<DieResult> = (0..roll.count)
            .map(|_| {
                let value = self.roll_die(&roll.sides);
                DieResult {
                    value,
                    rolls: vec![value],
                    dropped: false,
                }
            })
            .collect();

        // Apply modifiers in order: reroll -> explode -> keep/drop -> count
        let mut success_condition: Option<&Condition> = None;
        for modifier in &roll.modifiers {
            match modifier {
                Modifier::Reroll { once, condition } => {
                    self.apply_reroll(&mut dice, &roll.sides, *once, condition.as_ref())?;
                }
                Modifier::Explode { penetrating, condition } => {
                    self.apply_explode(&mut dice, &roll.sides, *penetrating, condition.as_ref())?;
                }
                Modifier::KeepHighest(n) => self.apply_keep_highest(&mut dice, *n),
                Modifier::KeepLowest(n) => self.apply_keep_lowest(&mut dice, *n),
                Modifier::DropHighest(n) => self.apply_drop_highest(&mut dice, *n),
                Modifier::DropLowest(n) => self.apply_drop_lowest(&mut dice, *n),
                Modifier::CountSuccesses(condition) => {
                    success_condition = Some(condition);
                }
            }
        }

        // Calculate total: count successes or sum values
        let total: i64 = if let Some(condition) = success_condition {
            dice.iter()
                .filter(|d| !d.dropped)
                .filter(|d| condition.compare.check(d.value, condition.value))
                .count() as i64
        } else {
            dice.iter().filter(|d| !d.dropped).map(|d| d.value).sum()
        };

        // Format the expression
        let expression = self.format_roll(roll, &dice, total, success_condition);

        Ok(RollResult {
            total,
            dice,
            expression,
        })
    }

    fn roll_die(&mut self, sides: &Sides) -> i64 {
        match sides {
            Sides::Number(n) => self.rng.roll(*n) as i64,
            Sides::Percent => self.rng.roll(100) as i64,
            Sides::Fudge => self.rng.roll(3) as i64 - 2, // -1, 0, 1
        }
    }

    fn apply_reroll(
        &mut self,
        dice: &mut [DieResult],
        sides: &Sides,
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
            while condition.compare.check(die.value, condition.value) {
                if reroll_count >= MAX_REROLLS {
                    return Err(Error::RerollLimit(MAX_REROLLS));
                }
                let new_value = self.roll_die(sides);
                die.rolls.push(new_value);
                die.value = new_value;
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
        sides: &Sides,
        penetrating: bool,
        condition: Option<&Condition>,
    ) -> Result<()> {
        let max_val = sides.count() as i64;
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

            let mut current_value = dice[i].value;
            let mut explode_count = 0;

            while condition.compare.check(current_value, condition.value) {
                if explode_count >= MAX_EXPLOSIONS {
                    return Err(Error::ExplodeLimit(MAX_EXPLOSIONS));
                }

                let new_value = self.roll_die(sides);

                // Penetrating: subtract 1 from added value (not from check)
                let added_value = if penetrating { new_value - 1 } else { new_value };

                dice[i].value += added_value;
                dice[i].rolls.push(new_value);

                current_value = new_value;
                explode_count += 1;
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
        indices.sort_by_key(|&i| dice[i].value);

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
        indices.sort_by_key(|&i| std::cmp::Reverse(dice[i].value));

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
        indices.sort_by_key(|&i| std::cmp::Reverse(dice[i].value));

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
        indices.sort_by_key(|&i| dice[i].value);

        for &i in indices.iter().take(n) {
            dice[i].dropped = true;
        }
    }

    fn format_roll(
        &self,
        roll: &Roll,
        dice: &[DieResult],
        total: i64,
        success_condition: Option<&Condition>,
    ) -> String {
        let sides_str = match roll.sides {
            Sides::Number(n) => n.to_string(),
            Sides::Percent => "%".to_string(),
            Sides::Fudge => "F".to_string(),
        };

        let modifiers_str: String = roll
            .modifiers
            .iter()
            .map(|m| match m {
                Modifier::KeepHighest(n) => format!("kh{}", n),
                Modifier::KeepLowest(n) => format!("kl{}", n),
                Modifier::DropHighest(n) => format!("dh{}", n),
                Modifier::DropLowest(n) => format!("dl{}", n),
                Modifier::Explode { penetrating, condition } => {
                    let mut s = "!".to_string();
                    if *penetrating {
                        s.push('p');
                    }
                    if let Some(c) = condition {
                        s.push_str(&format!("{}{}", c.compare, c.value));
                    }
                    s
                }
                Modifier::Reroll { once, condition } => {
                    let mut s = "r".to_string();
                    if *once {
                        s.push('o');
                    }
                    if let Some(c) = condition {
                        s.push_str(&format!("{}{}", c.compare, c.value));
                    }
                    s
                }
                Modifier::CountSuccesses(c) => {
                    format!("{}{}", c.compare, c.value)
                }
            })
            .collect();

        // Format dice, marking successes if counting
        let dice_str: String = dice
            .iter()
            .map(|d| {
                if d.dropped {
                    format!("({})", d.value)
                } else if let Some(condition) = success_condition {
                    if condition.compare.check(d.value, condition.value) {
                        format!("{}*", d.value) // Mark successes with *
                    } else {
                        d.value.to_string()
                    }
                } else {
                    d.value.to_string()
                }
            })
            .collect::<Vec<_>>()
            .join(", ");

        if success_condition.is_some() {
            let success_word = if total == 1 { "success" } else { "successes" };
            format!(
                "{}d{}{}[{}] = {} {}",
                roll.count, sides_str, modifiers_str, dice_str, total, success_word
            )
        } else {
            format!(
                "{}d{}{}[{}] = {}",
                roll.count, sides_str, modifiers_str, dice_str, total
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn test_evaluate_number() {
        let expr = Expr::Number(42);
        let result = evaluate(&expr).unwrap();
        assert_eq!(result.total, 42);
    }

    #[test]
    fn test_evaluate_basic_roll() {
        let roll = Roll {
            count: 2,
            sides: Sides::Number(6),
            modifiers: vec![],
        };
        let expr = Expr::Roll(roll);
        let mut rng = TestRng::new(vec![3, 4]);
        let result = evaluate_with_rng(&expr, &mut rng).unwrap();
        assert_eq!(result.total, 7);
    }

    #[test]
    fn test_evaluate_keep_highest() {
        let roll = Roll {
            count: 4,
            sides: Sides::Number(6),
            modifiers: vec![Modifier::KeepHighest(3)],
        };
        let expr = Expr::Roll(roll);
        let mut rng = TestRng::new(vec![1, 5, 3, 6]); // Should keep 5, 3, 6 = 14
        let result = evaluate_with_rng(&expr, &mut rng).unwrap();
        assert_eq!(result.total, 14);
    }

    #[test]
    fn test_evaluate_expression() {
        let expr = Expr::BinOp {
            op: Op::Add,
            left: Box::new(Expr::Roll(Roll {
                count: 2,
                sides: Sides::Number(6),
                modifiers: vec![],
            })),
            right: Box::new(Expr::Number(5)),
        };
        let mut rng = TestRng::new(vec![3, 4]);
        let result = evaluate_with_rng(&expr, &mut rng).unwrap();
        assert_eq!(result.total, 12); // 3 + 4 + 5
    }

    #[test]
    fn test_evaluate_fudge() {
        let roll = Roll {
            count: 4,
            sides: Sides::Fudge,
            modifiers: vec![],
        };
        let expr = Expr::Roll(roll);
        let mut rng = TestRng::new(vec![1, 2, 3, 2]); // -1, 0, 1, 0 = 0
        let result = evaluate_with_rng(&expr, &mut rng).unwrap();
        assert_eq!(result.total, 0);
    }

    #[test]
    fn test_evaluate_drop_lowest() {
        let roll = Roll {
            count: 4,
            sides: Sides::Number(6),
            modifiers: vec![Modifier::DropLowest(1)],
        };
        let expr = Expr::Roll(roll);
        let mut rng = TestRng::new(vec![1, 5, 3, 6]); // Drop 1, keep 5+3+6 = 14
        let result = evaluate_with_rng(&expr, &mut rng).unwrap();
        assert_eq!(result.total, 14);
    }

    #[test]
    fn test_evaluate_drop_highest() {
        let roll = Roll {
            count: 4,
            sides: Sides::Number(6),
            modifiers: vec![Modifier::DropHighest(1)],
        };
        let expr = Expr::Roll(roll);
        let mut rng = TestRng::new(vec![1, 5, 3, 6]); // Drop 6, keep 1+5+3 = 9
        let result = evaluate_with_rng(&expr, &mut rng).unwrap();
        assert_eq!(result.total, 9);
    }

    #[test]
    fn test_evaluate_count_successes() {
        let roll = Roll {
            count: 5,
            sides: Sides::Number(10),
            modifiers: vec![Modifier::CountSuccesses(Condition {
                compare: Compare::GreaterOrEqual,
                value: 8,
            })],
        };
        let expr = Expr::Roll(roll);
        let mut rng = TestRng::new(vec![10, 7, 8, 3, 9]); // 10, 8, 9 >= 8 = 3 successes
        let result = evaluate_with_rng(&expr, &mut rng).unwrap();
        assert_eq!(result.total, 3);
    }

    #[test]
    fn test_evaluate_count_successes_zero() {
        let roll = Roll {
            count: 3,
            sides: Sides::Number(6),
            modifiers: vec![Modifier::CountSuccesses(Condition {
                compare: Compare::Equal,
                value: 6,
            })],
        };
        let expr = Expr::Roll(roll);
        let mut rng = TestRng::new(vec![1, 2, 3]); // No 6s = 0 successes
        let result = evaluate_with_rng(&expr, &mut rng).unwrap();
        assert_eq!(result.total, 0);
    }

    #[test]
    fn test_evaluate_count_successes_output_format() {
        let roll = Roll {
            count: 4,
            sides: Sides::Number(10),
            modifiers: vec![Modifier::CountSuccesses(Condition {
                compare: Compare::GreaterOrEqual,
                value: 8,
            })],
        };
        let expr = Expr::Roll(roll);
        let mut rng = TestRng::new(vec![10, 5, 8, 3]); // 10*, 5, 8*, 3 = 2 successes
        let result = evaluate_with_rng(&expr, &mut rng).unwrap();
        assert!(result.expression.contains("successes"));
        assert!(result.expression.contains("10*")); // Success marked
        assert!(result.expression.contains("8*"));  // Success marked
    }

    #[test]
    fn test_evaluate_penetrating_explode() {
        let roll = Roll {
            count: 1,
            sides: Sides::Number(6),
            modifiers: vec![Modifier::Explode {
                penetrating: true,
                condition: None,
            }],
        };
        let expr = Expr::Roll(roll);
        // Rolls: 6 (explode), 6 (explode), 4 (stop)
        // Added: 6 + (6-1) + (4-1) = 6 + 5 + 3 = 14
        let mut rng = TestRng::new(vec![6, 6, 4]);
        let result = evaluate_with_rng(&expr, &mut rng).unwrap();
        assert_eq!(result.total, 14);
    }

    #[test]
    fn test_evaluate_penetrating_explode_no_explosion() {
        let roll = Roll {
            count: 1,
            sides: Sides::Number(6),
            modifiers: vec![Modifier::Explode {
                penetrating: true,
                condition: None,
            }],
        };
        let expr = Expr::Roll(roll);
        // Roll: 4 (no explosion)
        // Total: 4 (no -1 because no explosion occurred)
        let mut rng = TestRng::new(vec![4]);
        let result = evaluate_with_rng(&expr, &mut rng).unwrap();
        assert_eq!(result.total, 4);
    }
}
