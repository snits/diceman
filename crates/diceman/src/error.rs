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
}

pub type Result<T> = std::result::Result<T, Error>;
