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

    #[error("Invalid dice sides: {0}")]
    InvalidDiceSides(u32),

    #[error("Explode limit exceeded (max {0} explosions)")]
    ExplodeLimit(u32),

    #[error("Reroll limit exceeded (max {0} rerolls)")]
    RerollLimit(u32),

    #[error("Division by zero")]
    DivisionByZero,
}

pub type Result<T> = std::result::Result<T, Error>;
