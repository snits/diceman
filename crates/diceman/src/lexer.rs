// ABOUTME: Lexer for dice notation expressions.
// ABOUTME: Tokenizes strings like "4d6kh3+5" into a stream of tokens.

use crate::error::{Error, Result};

/// A token in the dice notation language.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Token {
    /// A numeric literal.
    Number(u32),
    /// The 'd' dice separator (lowercase, standard roll).
    D,
    /// The 'D' dice separator (uppercase, digit dice like D66).
    DigitD,
    /// Percent sign for d%.
    Percent,
    /// 'F' for fudge dice.
    Fudge,
    /// 'Marvel' for Marvel Multiverse dice.
    Marvel,
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
    /// Penetrating modifier: 'p'.
    P,
    /// Critical success marker: 'cs'.
    CritSuccess,
    /// Critical failure marker: 'cf'.
    CritFail,
    /// Equal comparison: '='.
    Eq,
    /// Less than: '<'.
    Lt,
    /// Greater than: '>'.
    Gt,
    /// Edge modifier: 'e' (Marvel reroll lowest, keep better by rank).
    Edge,
    /// Trouble modifier: 't' (Marvel reroll highest, keep worse by rank).
    Trouble,
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
            'd' => {
                self.chars.next();
                Ok(Token::D)
            }
            'D' => {
                self.chars.next();
                Ok(Token::DigitD)
            }
            '%' => {
                self.chars.next();
                Ok(Token::Percent)
            }
            'F' | 'f' => {
                self.chars.next();
                Ok(Token::Fudge)
            }
            'm' | 'M' => self.word("marvel", Token::Marvel),
            'e' | 'E' => {
                self.chars.next();
                Ok(Token::Edge)
            }
            't' | 'T' => {
                self.chars.next();
                Ok(Token::Trouble)
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
            'p' | 'P' => {
                self.chars.next();
                Ok(Token::P)
            }
            'c' | 'C' => {
                self.chars.next(); // consume 'c'
                if let Some(&(next_pos, next_ch)) = self.chars.peek() {
                    match next_ch {
                        's' | 'S' => {
                            self.chars.next();
                            Ok(Token::CritSuccess)
                        }
                        'f' | 'F' => {
                            self.chars.next();
                            Ok(Token::CritFail)
                        }
                        _ => Err(Error::UnexpectedChar(next_ch, next_pos)),
                    }
                } else {
                    Err(Error::UnexpectedChar(ch, pos))
                }
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

    fn word(&mut self, expected: &str, token: Token) -> Result<Token> {
        for expected_ch in expected.chars() {
            match self.chars.peek().copied() {
                Some((_, ch)) if ch.eq_ignore_ascii_case(&expected_ch) => {
                    self.chars.next();
                }
                Some((pos, ch)) => return Err(Error::UnexpectedChar(ch, pos)),
                None => return Err(Error::UnexpectedEof),
            }
        }

        Ok(token)
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

    #[test]
    fn test_penetrating() {
        let mut lexer = Lexer::new("1d6!p");
        assert_eq!(lexer.next_token().unwrap(), Token::Number(1));
        assert_eq!(lexer.next_token().unwrap(), Token::D);
        assert_eq!(lexer.next_token().unwrap(), Token::Number(6));
        assert_eq!(lexer.next_token().unwrap(), Token::Explode);
        assert_eq!(lexer.next_token().unwrap(), Token::P);
        assert_eq!(lexer.next_token().unwrap(), Token::Eof);
    }

    #[test]
    fn test_crit_success() {
        let mut lexer = Lexer::new("1d20cs20");
        assert_eq!(lexer.next_token().unwrap(), Token::Number(1));
        assert_eq!(lexer.next_token().unwrap(), Token::D);
        assert_eq!(lexer.next_token().unwrap(), Token::Number(20));
        assert_eq!(lexer.next_token().unwrap(), Token::CritSuccess);
        assert_eq!(lexer.next_token().unwrap(), Token::Number(20));
        assert_eq!(lexer.next_token().unwrap(), Token::Eof);
    }

    #[test]
    fn test_crit_failure() {
        let mut lexer = Lexer::new("1d20cf1");
        assert_eq!(lexer.next_token().unwrap(), Token::Number(1));
        assert_eq!(lexer.next_token().unwrap(), Token::D);
        assert_eq!(lexer.next_token().unwrap(), Token::Number(20));
        assert_eq!(lexer.next_token().unwrap(), Token::CritFail);
        assert_eq!(lexer.next_token().unwrap(), Token::Number(1));
        assert_eq!(lexer.next_token().unwrap(), Token::Eof);
    }

    #[test]
    fn test_marvel_word_token_is_case_insensitive() {
        for input in ["3dMarvel", "3dmarvel", "3dMARVEL"] {
            let mut lexer = Lexer::new(input);
            assert_eq!(lexer.next_token().unwrap(), Token::Number(3));
            assert_eq!(lexer.next_token().unwrap(), Token::D);
            assert_eq!(lexer.next_token().unwrap(), Token::Marvel);
            assert_eq!(lexer.next_token().unwrap(), Token::Eof);
        }
    }

    #[test]
    fn test_single_m_is_not_marvel_token() {
        let mut lexer = Lexer::new("3dm");
        assert_eq!(lexer.next_token().unwrap(), Token::Number(3));
        assert_eq!(lexer.next_token().unwrap(), Token::D);
        assert!(matches!(lexer.next_token(), Err(Error::UnexpectedEof)));
    }

    #[test]
    fn test_truncated_marvel_keyword_errors() {
        let mut lexer = Lexer::new("3dMarve");
        assert_eq!(lexer.next_token().unwrap(), Token::Number(3));
        assert_eq!(lexer.next_token().unwrap(), Token::D);
        assert!(matches!(lexer.next_token(), Err(Error::UnexpectedEof)));
    }

    #[test]
    fn test_bare_c_error() {
        let mut lexer = Lexer::new("c");
        let result = lexer.next_token();
        assert!(result.is_err());
        if let Err(Error::UnexpectedChar(ch, pos)) = result {
            assert_eq!(ch, 'c');
            assert_eq!(pos, 0);
        } else {
            panic!("Expected UnexpectedChar error");
        }
    }

    #[test]
    fn test_lowercase_d_token() {
        let mut lexer = Lexer::new("d6");
        assert_eq!(lexer.next_token().unwrap(), Token::D);
        assert_eq!(lexer.next_token().unwrap(), Token::Number(6));
    }

    #[test]
    fn test_uppercase_d_token() {
        let mut lexer = Lexer::new("D66");
        assert_eq!(lexer.next_token().unwrap(), Token::DigitD);
        assert_eq!(lexer.next_token().unwrap(), Token::Number(66));
    }

    #[test]
    fn test_mixed_case_expression() {
        let mut lexer = Lexer::new("2d6");
        assert_eq!(lexer.next_token().unwrap(), Token::Number(2));
        assert_eq!(lexer.next_token().unwrap(), Token::D);
        assert_eq!(lexer.next_token().unwrap(), Token::Number(6));
    }

    #[test]
    fn test_invalid_c_sequence_error() {
        let mut lexer = Lexer::new("1d20cx");
        assert_eq!(lexer.next_token().unwrap(), Token::Number(1));
        assert_eq!(lexer.next_token().unwrap(), Token::D);
        assert_eq!(lexer.next_token().unwrap(), Token::Number(20));
        let result = lexer.next_token();
        assert!(result.is_err());
        if let Err(Error::UnexpectedChar(ch, pos)) = result {
            assert_eq!(ch, 'x');
            assert_eq!(pos, 5);
        } else {
            panic!("Expected UnexpectedChar error for 'x' at position 5");
        }
    }

    #[test]
    fn test_edge_token_case_insensitive() {
        for ch in ["e", "E"] {
            let mut lexer = Lexer::new(ch);
            assert_eq!(lexer.next_token().unwrap(), Token::Edge);
            assert_eq!(lexer.next_token().unwrap(), Token::Eof);
        }
    }

    #[test]
    fn test_trouble_token_case_insensitive() {
        for ch in ["t", "T"] {
            let mut lexer = Lexer::new(ch);
            assert_eq!(lexer.next_token().unwrap(), Token::Trouble);
            assert_eq!(lexer.next_token().unwrap(), Token::Eof);
        }
    }

    #[test]
    fn test_marvel_edge_notation_lexes() {
        let mut lexer = Lexer::new("3dMarvele2");
        assert_eq!(lexer.next_token().unwrap(), Token::Number(3));
        assert_eq!(lexer.next_token().unwrap(), Token::D);
        assert_eq!(lexer.next_token().unwrap(), Token::Marvel);
        assert_eq!(lexer.next_token().unwrap(), Token::Edge);
        assert_eq!(lexer.next_token().unwrap(), Token::Number(2));
        assert_eq!(lexer.next_token().unwrap(), Token::Eof);
    }
}
