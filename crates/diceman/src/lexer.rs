// ABOUTME: Lexer for dice notation expressions.
// ABOUTME: Tokenizes strings like "4d6kh3+5" into a stream of tokens.

use crate::error::{Error, Result};

/// A token in the dice notation language.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Token {
    /// A numeric literal.
    Number(u32),
    /// The 'd' or 'D' dice separator.
    D,
    /// Percent sign for d%.
    Percent,
    /// 'F' for fudge dice.
    Fudge,
    /// Addition operator.
    Plus,
    /// Subtraction operator.
    Minus,
    /// Multiplication operator.
    Star,
    /// Division operator.
    Slash,
    /// Left parenthesis.
    LParen,
    /// Right parenthesis.
    RParen,
    /// Keep modifier: 'k'.
    K,
    /// High modifier: 'h'.
    H,
    /// Low modifier: 'l'.
    L,
    /// Explode modifier: '!'.
    Explode,
    /// Reroll modifier: 'r'.
    R,
    /// Once modifier: 'o'.
    O,
    /// Equal comparison: '='.
    Eq,
    /// Less than: '<'.
    Lt,
    /// Greater than: '>'.
    Gt,
    /// End of input.
    Eof,
}

/// A lexer for dice notation.
pub struct Lexer<'a> {
    #[allow(dead_code)]
    input: &'a str,
    chars: std::iter::Peekable<std::str::CharIndices<'a>>,
    pos: usize,
}

impl<'a> Lexer<'a> {
    /// Create a new lexer for the given input.
    pub fn new(input: &'a str) -> Self {
        Self {
            input,
            chars: input.char_indices().peekable(),
            pos: 0,
        }
    }

    /// Get the current position in the input.
    pub fn pos(&self) -> usize {
        self.pos
    }

    /// Peek at the next token without consuming it.
    pub fn peek(&mut self) -> Result<Token> {
        let saved_chars = self.chars.clone();
        let saved_pos = self.pos;
        let token = self.next_token()?;
        self.chars = saved_chars;
        self.pos = saved_pos;
        Ok(token)
    }

    /// Get the next token from the input.
    pub fn next_token(&mut self) -> Result<Token> {
        self.skip_whitespace();

        let Some(&(pos, ch)) = self.chars.peek() else {
            return Ok(Token::Eof);
        };

        self.pos = pos;

        match ch {
            '0'..='9' => self.number(),
            'd' | 'D' => {
                self.chars.next();
                Ok(Token::D)
            }
            '%' => {
                self.chars.next();
                Ok(Token::Percent)
            }
            'F' | 'f' => {
                self.chars.next();
                Ok(Token::Fudge)
            }
            '+' => {
                self.chars.next();
                Ok(Token::Plus)
            }
            '-' => {
                self.chars.next();
                Ok(Token::Minus)
            }
            '*' => {
                self.chars.next();
                Ok(Token::Star)
            }
            '/' => {
                self.chars.next();
                Ok(Token::Slash)
            }
            '(' => {
                self.chars.next();
                Ok(Token::LParen)
            }
            ')' => {
                self.chars.next();
                Ok(Token::RParen)
            }
            'k' | 'K' => {
                self.chars.next();
                Ok(Token::K)
            }
            'h' | 'H' => {
                self.chars.next();
                Ok(Token::H)
            }
            'l' | 'L' => {
                self.chars.next();
                Ok(Token::L)
            }
            '!' => {
                self.chars.next();
                Ok(Token::Explode)
            }
            'r' | 'R' => {
                self.chars.next();
                Ok(Token::R)
            }
            'o' | 'O' => {
                self.chars.next();
                Ok(Token::O)
            }
            '=' => {
                self.chars.next();
                Ok(Token::Eq)
            }
            '<' => {
                self.chars.next();
                Ok(Token::Lt)
            }
            '>' => {
                self.chars.next();
                Ok(Token::Gt)
            }
            _ => Err(Error::UnexpectedChar(ch, pos)),
        }
    }

    fn skip_whitespace(&mut self) {
        while let Some(&(_, ch)) = self.chars.peek() {
            if ch.is_whitespace() {
                self.chars.next();
            } else {
                break;
            }
        }
    }

    fn number(&mut self) -> Result<Token> {
        let mut value: u32 = 0;

        while let Some(&(_, ch)) = self.chars.peek() {
            if let Some(digit) = ch.to_digit(10) {
                self.chars.next();
                value = value.saturating_mul(10).saturating_add(digit);
            } else {
                break;
            }
        }

        Ok(Token::Number(value))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_roll() {
        let mut lexer = Lexer::new("2d6");
        assert_eq!(lexer.next_token().unwrap(), Token::Number(2));
        assert_eq!(lexer.next_token().unwrap(), Token::D);
        assert_eq!(lexer.next_token().unwrap(), Token::Number(6));
        assert_eq!(lexer.next_token().unwrap(), Token::Eof);
    }

    #[test]
    fn test_roll_with_modifier() {
        let mut lexer = Lexer::new("4d6kh3");
        assert_eq!(lexer.next_token().unwrap(), Token::Number(4));
        assert_eq!(lexer.next_token().unwrap(), Token::D);
        assert_eq!(lexer.next_token().unwrap(), Token::Number(6));
        assert_eq!(lexer.next_token().unwrap(), Token::K);
        assert_eq!(lexer.next_token().unwrap(), Token::H);
        assert_eq!(lexer.next_token().unwrap(), Token::Number(3));
        assert_eq!(lexer.next_token().unwrap(), Token::Eof);
    }

    #[test]
    fn test_expression() {
        let mut lexer = Lexer::new("2d6 + 5");
        assert_eq!(lexer.next_token().unwrap(), Token::Number(2));
        assert_eq!(lexer.next_token().unwrap(), Token::D);
        assert_eq!(lexer.next_token().unwrap(), Token::Number(6));
        assert_eq!(lexer.next_token().unwrap(), Token::Plus);
        assert_eq!(lexer.next_token().unwrap(), Token::Number(5));
        assert_eq!(lexer.next_token().unwrap(), Token::Eof);
    }

    #[test]
    fn test_explode() {
        let mut lexer = Lexer::new("1d6!");
        assert_eq!(lexer.next_token().unwrap(), Token::Number(1));
        assert_eq!(lexer.next_token().unwrap(), Token::D);
        assert_eq!(lexer.next_token().unwrap(), Token::Number(6));
        assert_eq!(lexer.next_token().unwrap(), Token::Explode);
        assert_eq!(lexer.next_token().unwrap(), Token::Eof);
    }

    #[test]
    fn test_percent_and_fudge() {
        let mut lexer = Lexer::new("d% dF");
        assert_eq!(lexer.next_token().unwrap(), Token::D);
        assert_eq!(lexer.next_token().unwrap(), Token::Percent);
        assert_eq!(lexer.next_token().unwrap(), Token::D);
        assert_eq!(lexer.next_token().unwrap(), Token::Fudge);
        assert_eq!(lexer.next_token().unwrap(), Token::Eof);
    }
}
