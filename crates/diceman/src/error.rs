// ABOUTME: Error types for the diceman library.
// ABOUTME: Covers lexing, parsing, and evaluation errors.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("Unexpected character '{0}' at position {1}")]
    UnexpectedChar(char, usize),

    #[error("Unexpected end of input")]
    UnexpectedEof,

    #[error("Expected {expected}, found {found}")]
    Expected { expected: String, found: String },

    #[error("Invalid dice count: {0}")]
    InvalidDiceCount(u32),

    #[error("Invalid trial count: {0}")]
    InvalidTrialCount(usize),

    #[error("Invalid die kind: {0}")]
    InvalidDieKind(u32),

    #[error("Explode limit exceeded (max {0} explosions)")]
    ExplodeLimit(u32),

    #[error("Reroll limit exceeded (max {0} rerolls)")]
    RerollLimit(u32),

    #[error("Division by zero")]
    DivisionByZero,

    #[error("Duplicate critical marker: {0}")]
    DuplicateCritMarker(String),

    #[error("Critical markers cannot be combined with success counting")]
    CritWithSuccessCounting,

    #[error("Invalid digit dice value: {0} (all digits must be the same, e.g., D66, D444, D88)")]
    InvalidDigitDice(u32),

    #[error("Invalid Marvel roll: {0}")]
    InvalidMarvelRoll(String),

    #[error("Invalid narrative roll: {0}")]
    InvalidNarrativeRoll(String),

    #[error("{0} count must be at least 1")]
    ZeroMarvelCount(&'static str),

    #[error("dice result is not numeric and cannot be used in arithmetic")]
    NonNumericOutcome,

    #[error("simulation statistics overflowed accumulating trial totals")]
    SimulationOverflow,
}

pub type Result<T> = std::result::Result<T, Error>;

#[cfg(test)]
mod tests {
    use super::*;

    // Each of these pins the exact Display text, not just the variant --
    // a mutation pass found 9 of 10 tested variants' #[error("...")]
    // strings could be replaced with a placeholder like "MUTANT" while the
    // full workspace suite stayed green, because nothing asserted on
    // `.to_string()` output anywhere.

    #[test]
    fn unexpected_char_display() {
        assert_eq!(
            Error::UnexpectedChar('x', 5).to_string(),
            "Unexpected character 'x' at position 5"
        );
    }

    #[test]
    fn unexpected_eof_display() {
        assert_eq!(Error::UnexpectedEof.to_string(), "Unexpected end of input");
    }

    #[test]
    fn expected_display() {
        assert_eq!(
            Error::Expected {
                expected: "a number".to_string(),
                found: "'d'".to_string(),
            }
            .to_string(),
            "Expected a number, found 'd'"
        );
    }

    #[test]
    fn invalid_dice_count_display() {
        assert_eq!(
            Error::InvalidDiceCount(0).to_string(),
            "Invalid dice count: 0"
        );
    }

    #[test]
    fn invalid_trial_count_display() {
        assert_eq!(
            Error::InvalidTrialCount(0).to_string(),
            "Invalid trial count: 0"
        );
    }

    #[test]
    fn invalid_die_kind_display() {
        assert_eq!(Error::InvalidDieKind(0).to_string(), "Invalid die kind: 0");
    }

    #[test]
    fn explode_limit_display() {
        assert_eq!(
            Error::ExplodeLimit(100).to_string(),
            "Explode limit exceeded (max 100 explosions)"
        );
    }

    #[test]
    fn reroll_limit_display() {
        assert_eq!(
            Error::RerollLimit(100).to_string(),
            "Reroll limit exceeded (max 100 rerolls)"
        );
    }

    #[test]
    fn division_by_zero_display() {
        assert_eq!(Error::DivisionByZero.to_string(), "Division by zero");
    }

    #[test]
    fn duplicate_crit_marker_display() {
        assert_eq!(
            Error::DuplicateCritMarker("cs".to_string()).to_string(),
            "Duplicate critical marker: cs"
        );
    }

    #[test]
    fn crit_with_success_counting_display() {
        assert_eq!(
            Error::CritWithSuccessCounting.to_string(),
            "Critical markers cannot be combined with success counting"
        );
    }

    #[test]
    fn invalid_digit_dice_display() {
        // The user-guidance clause ("all digits must be the same...") is
        // the entire value of this message -- it can be deleted green
        // without this assertion.
        assert_eq!(
            Error::InvalidDigitDice(67).to_string(),
            "Invalid digit dice value: 67 (all digits must be the same, e.g., D66, D444, D88)"
        );
    }

    #[test]
    fn invalid_marvel_roll_display() {
        assert_eq!(
            Error::InvalidMarvelRoll("bad marvel roll".to_string()).to_string(),
            "Invalid Marvel roll: bad marvel roll"
        );
    }

    #[test]
    fn invalid_narrative_roll_display() {
        assert_eq!(
            Error::InvalidNarrativeRoll("bad narrative roll".to_string()).to_string(),
            "Invalid narrative roll: bad narrative roll"
        );
    }

    #[test]
    fn zero_marvel_count_display() {
        assert_eq!(
            Error::ZeroMarvelCount("edge").to_string(),
            "edge count must be at least 1"
        );
    }

    #[test]
    fn non_numeric_outcome_display() {
        assert_eq!(
            Error::NonNumericOutcome.to_string(),
            "dice result is not numeric and cannot be used in arithmetic"
        );
    }

    #[test]
    fn simulation_overflow_display() {
        assert_eq!(
            Error::SimulationOverflow.to_string(),
            "simulation statistics overflowed accumulating trial totals"
        );
    }
}
