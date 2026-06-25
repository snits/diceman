// ABOUTME: Core library for parsing and rolling TTRPG dice notation.
// ABOUTME: Supports Roll20-style notation with modifiers, simulation, and RNG abstraction.

//! # Diceman
//!
//! A dice notation parser and roller for tabletop RPGs.
//!
//! ## Quick Start
//!
//! ```
//! use diceman::{roll, simulate};
//!
//! // Roll dice
//! let result = roll("4d6kh3").unwrap();
//! println!("{}", result);  // e.g., "4d6kh3[6, 5, 4, (1)] = 15"
//!
//! // Simulate probability distribution
//! let sim = simulate("2d6", 10000).unwrap();
//! println!("Mean: {:.2}", sim.mean);  // ~7.0
//! ```
//!
//! ## Supported Notation
//!
//! - Basic rolls: `2d6`, `1d20`, `d%`, `4dF`
//! - Arithmetic: `2d6 + 5`, `(1d6 + 2) * 3`
//! - Keep highest/lowest: `4d6kh3`, `2d20kl1`
//! - Drop highest/lowest: `4d6dh1`, `4d6dl1`
//! - Exploding dice: `1d6!`, `1d6!>5`
//! - Reroll: `1d6r`, `1d6r<3`

pub mod ast;
pub mod error;
mod format;
pub mod lexer;
pub mod parser;
pub mod roller;
pub mod sim;

pub use ast::{Compare, Condition, Expr, Modifier, Op, Roll, DieKind};
pub use error::{Error, Result};
pub use roller::{DieResult, FastRng, Rng, RngCheckpoint, RollResult};
pub use sim::{simulate, simulate_seeded, SimResult};

/// Parse and roll a dice expression in one step.
///
/// # Examples
///
/// ```
/// let result = diceman::roll("2d6 + 5").unwrap();
/// println!("Total: {}", result.total);
/// println!("Expression: {}", result.expression);
/// ```
pub fn roll(expr: &str) -> Result<RollResult> {
    let parsed = parser::parse(expr)?;
    roller::evaluate(&parsed)
}

/// Parse and roll with a custom RNG.
///
/// Useful for testing or when you need reproducible results.
///
/// # Examples
///
/// ```
/// use diceman::{roll_with_rng, FastRng};
///
/// let mut rng = FastRng::with_seed(42);
/// let result = roll_with_rng("2d6", &mut rng).unwrap();
/// ```
pub fn roll_with_rng(expr: &str, rng: &mut impl Rng) -> Result<RollResult> {
    let parsed = parser::parse(expr)?;
    roller::evaluate_with_rng(&parsed, rng)
}

/// Parse a dice expression without rolling.
///
/// Returns the AST representation of the expression.
///
/// # Examples
///
/// ```
/// use diceman::{parse, Expr, Roll, DieKind, Modifier};
///
/// let expr = diceman::parse("4d6kh3").unwrap();
/// match expr {
///     Expr::Roll(roll) => {
///         assert_eq!(roll.count, 4);
///         assert_eq!(roll.kind, DieKind::Number(6));
///     }
///     _ => panic!("Expected a roll"),
/// }
/// ```
pub fn parse(input: &str) -> Result<Expr> {
    parser::parse(input)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Deterministic RNG for testing with pre-defined values.
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
    fn test_roll_basic() {
        let result = roll("2d6").unwrap();
        assert!(result.total >= 2 && result.total <= 12);
    }

    #[test]
    fn test_roll_with_modifier() {
        let result = roll("4d6kh3").unwrap();
        assert!(result.total >= 3 && result.total <= 18);
    }

    #[test]
    fn test_roll_expression() {
        let result = roll("2d6 + 5").unwrap();
        assert!(result.total >= 7 && result.total <= 17);
    }

    #[test]
    fn test_roll_seeded() {
        let mut rng = FastRng::with_seed(42);
        let result1 = roll_with_rng("2d6", &mut rng).unwrap();

        let mut rng = FastRng::with_seed(42);
        let result2 = roll_with_rng("2d6", &mut rng).unwrap();

        assert_eq!(result1.total, result2.total);
    }

    #[test]
    fn rng_checkpoint_is_available_from_crate_root() {
        let mut rng = FastRng::with_seed(123);
        let checkpoint: RngCheckpoint = rng.checkpoint();
        let expected = rng.roll(6);

        rng.restore(checkpoint);

        assert_eq!(rng.roll(6), expected);
    }

    #[test]
    fn test_parse() {
        let expr = parse("4d6kh3").unwrap();
        match expr {
            Expr::Roll(roll) => {
                assert_eq!(roll.count, 4);
                assert_eq!(roll.kind, DieKind::Number(6));
                assert_eq!(roll.modifiers.len(), 1);
            }
            _ => panic!("Expected a roll"),
        }
    }

    #[test]
    fn test_simulate_integration() {
        let result = simulate("2d6", 1000).unwrap();
        assert!(result.min >= 2);
        assert!(result.max <= 12);
        assert!((result.mean - 7.0).abs() < 0.5);
    }

    #[test]
    fn test_digit_dice_integration() {
        // D66 should produce values from 11-66 with no 7-9 or 0 digits
        let result = roll("D66").unwrap();
        assert!(result.total >= 11 && result.total <= 66);
        // Each digit should be 1-6
        let tens = result.total / 10;
        let ones = result.total % 10;
        assert!((1..=6).contains(&tens));
        assert!((1..=6).contains(&ones));
    }

    #[test]
    fn test_digit_dice_simulation() {
        let result = simulate("D66", 10000).unwrap();
        assert!(result.min >= 11);
        assert!(result.max <= 66);
        // D66 has 36 outcomes, mean should be around 38.5
        // (average of 1-6 for tens = 3.5, for ones = 3.5, so 35 + 3.5 = 38.5)
        assert!((result.mean - 38.5).abs() < 1.0);
    }

    #[test]
    fn test_crit_markers_integration() {
        // Test crit success marker
        let mut rng = TestRng::new(vec![20]);
        let result = roll_with_rng("1d20cs20cf1", &mut rng).unwrap();
        assert_eq!(result.total, 20);
        assert!(
            result.expression.contains("20**"),
            "Expected crit success marker ** in output: {}",
            result.expression
        );

        // Test crit failure marker
        let mut rng = TestRng::new(vec![1]);
        let result = roll_with_rng("1d20cs20cf1", &mut rng).unwrap();
        assert_eq!(result.total, 1);
        assert!(
            result.expression.contains("1*"),
            "Expected crit failure marker * in output: {}",
            result.expression
        );

        // Test normal roll (no markers)
        let mut rng = TestRng::new(vec![10]);
        let result = roll_with_rng("1d20cs20cf1", &mut rng).unwrap();
        assert_eq!(result.total, 10);
        assert!(
            !result.expression.contains("10*") && !result.expression.contains("10**"),
            "Expected no crit markers for normal roll: {}",
            result.expression
        );

        // Test expanded crit range
        let mut rng = TestRng::new(vec![19]);
        let result = roll_with_rng("1d20cs>=19cf1", &mut rng).unwrap();
        assert_eq!(result.total, 19);
        assert!(
            result.expression.contains("19**"),
            "Expected crit success marker ** for 19 in expanded range: {}",
            result.expression
        );
    }

    // --- Deterministic end-to-end tests ---

    #[test]
    fn test_e2e_reroll_keep_highest() {
        // 4d6rkh3: roll 4d6, reroll 1s, keep highest 3
        // Rolls: die0=1 (rerolled→4), die1=5, die2=3, die3=6
        // After reroll: [4, 5, 3, 6], keep highest 3: 6+5+4=15
        let mut rng = TestRng::new(vec![1, 5, 3, 6, 4]);
        let result = roll_with_rng("4d6rkh3", &mut rng).unwrap();
        assert_eq!(result.total, 15);
    }

    #[test]
    fn test_e2e_explode_keep() {
        // 2d6!kh2: roll 2d6, explode on 6, keep highest 2
        // Rolls: die0=6 (explodes→new die=4), die1=3
        // Dice: [6, 3, 4], keep highest 2: 6+4=10
        let mut rng = TestRng::new(vec![6, 3, 4]);
        let result = roll_with_rng("2d6!kh2", &mut rng).unwrap();
        assert_eq!(result.total, 10);
    }

    #[test]
    fn test_e2e_arithmetic_with_rolls() {
        // 2d6 + 1d4 * 2: arithmetic with standard precedence
        // Rolls: 2d6→[3, 4], 1d4→[2]
        // Total: (3+4) + (2*2) = 11
        let mut rng = TestRng::new(vec![3, 4, 2]);
        let result = roll_with_rng("2d6 + 1d4 * 2", &mut rng).unwrap();
        assert_eq!(result.total, 11);
    }

    #[test]
    fn test_e2e_negative_number() {
        // -5: unary negation (parsed as 0 - 5)
        let mut rng = TestRng::new(vec![1]); // won't be used
        let result = roll_with_rng("-5", &mut rng).unwrap();
        assert_eq!(result.total, -5);
    }

    #[test]
    fn test_e2e_grouped_expression() {
        // (1d6 + 2) * 3: grouped arithmetic
        // Rolls: 1d6→[4]
        // Total: (4+2)*3 = 18
        let mut rng = TestRng::new(vec![4]);
        let result = roll_with_rng("(1d6 + 2) * 3", &mut rng).unwrap();
        assert_eq!(result.total, 18);
    }

    #[test]
    fn test_e2e_parse_error() {
        // Empty string should produce a parse error
        let err = roll("").unwrap_err();
        assert!(matches!(err, Error::Expected { .. }));
    }
}
