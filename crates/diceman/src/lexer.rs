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
    /// Pool-union operator for narrative rolls: '&'.
    Ampersand,
    /// 'Ability' narrative die word.
    Ability,
    /// 'Boost' narrative die word.
    Boost,
    /// 'Setback' narrative die word.
    Setback,
    /// 'Difficulty' narrative die word.
    Difficulty,
    /// 'Proficiency' narrative die word.
    Proficiency,
    /// 'Challenge' narrative die word.
    Challenge,
    /// 'Force' narrative die word.
    Force,
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
            'd' => self.word_or_fallback("ifficulty", Token::Difficulty, Token::D),
            'D' => {
                if self
                    .peek_next()
                    .is_some_and(|c| c.eq_ignore_ascii_case(&'i'))
                {
                    self.word("difficulty", Token::Difficulty)
                } else {
                    self.chars.next();
                    Ok(Token::DigitD)
                }
            }
            '%' => {
                self.chars.next();
                Ok(Token::Percent)
            }
            'F' | 'f' => {
                if self
                    .peek_next()
                    .is_some_and(|c| c.eq_ignore_ascii_case(&'o'))
                {
                    self.word("force", Token::Force)
                } else {
                    self.chars.next();
                    Ok(Token::Fudge)
                }
            }
            'm' | 'M' => self.word("marvel", Token::Marvel),
            'a' | 'A' => self.word("ability", Token::Ability),
            'b' | 'B' => self.word("boost", Token::Boost),
            's' | 'S' => self.word("setback", Token::Setback),
            '&' => {
                self.chars.next();
                Ok(Token::Ampersand)
            }
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
            'p' | 'P' => self.word_or_fallback("roficiency", Token::Proficiency, Token::P),
            'c' | 'C' => {
                if self
                    .peek_next()
                    .is_some_and(|c| c.eq_ignore_ascii_case(&'h'))
                {
                    self.word("challenge", Token::Challenge)
                } else {
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

    /// Peek the character immediately after the current (not-yet-consumed)
    /// character, without consuming anything.
    fn peek_next(&self) -> Option<char> {
        let mut iter = self.chars.clone();
        iter.next();
        iter.next().map(|(_, c)| c)
    }

    /// Consume the current anchor character, then attempt to match `rest`
    /// case-insensitively on a cloned iterator. On a full match, returns
    /// `token` with the whole word consumed. On any mismatch (including
    /// running out of input), restores the iterator and returns `fallback`
    /// having consumed only the anchor character.
    fn word_or_fallback(&mut self, rest: &str, token: Token, fallback: Token) -> Result<Token> {
        let saved_chars = self.chars.clone();
        self.chars.next(); // consume the anchor character
        for expected_ch in rest.chars() {
            match self.chars.peek().copied() {
                Some((_, ch)) if ch.eq_ignore_ascii_case(&expected_ch) => {
                    self.chars.next();
                }
                _ => {
                    self.chars = saved_chars;
                    self.chars.next();
                    return Ok(fallback);
                }
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

    // --- Narrative notation disambiguation matrix (regression-locked) ---

    #[test]
    fn test_penetrating_explode_reroll_regression() {
        // 1d6!pr must still lex as penetrating explode + reroll, not as a
        // 'p' word lookahead toward "Proficiency".
        let mut lexer = Lexer::new("1d6!pr");
        assert_eq!(lexer.next_token().unwrap(), Token::Number(1));
        assert_eq!(lexer.next_token().unwrap(), Token::D);
        assert_eq!(lexer.next_token().unwrap(), Token::Number(6));
        assert_eq!(lexer.next_token().unwrap(), Token::Explode);
        assert_eq!(lexer.next_token().unwrap(), Token::P);
        assert_eq!(lexer.next_token().unwrap(), Token::R);
        assert_eq!(lexer.next_token().unwrap(), Token::Eof);
    }

    #[test]
    fn test_penetrating_explode_alone_regression() {
        let mut lexer = Lexer::new("1d6!p");
        assert_eq!(lexer.next_token().unwrap(), Token::Number(1));
        assert_eq!(lexer.next_token().unwrap(), Token::D);
        assert_eq!(lexer.next_token().unwrap(), Token::Number(6));
        assert_eq!(lexer.next_token().unwrap(), Token::Explode);
        assert_eq!(lexer.next_token().unwrap(), Token::P);
        assert_eq!(lexer.next_token().unwrap(), Token::Eof);
    }

    #[test]
    fn test_fudge_regression() {
        let mut lexer = Lexer::new("1dF");
        assert_eq!(lexer.next_token().unwrap(), Token::Number(1));
        assert_eq!(lexer.next_token().unwrap(), Token::D);
        assert_eq!(lexer.next_token().unwrap(), Token::Fudge);
        assert_eq!(lexer.next_token().unwrap(), Token::Eof);
    }

    #[test]
    fn test_fudge_word_not_supported() {
        // 1dF then trailing "udge" is not a recognized word; current-style
        // error, not invented "fudge" word support.
        let mut lexer = Lexer::new("1dfudge");
        assert_eq!(lexer.next_token().unwrap(), Token::Number(1));
        assert_eq!(lexer.next_token().unwrap(), Token::D);
        assert_eq!(lexer.next_token().unwrap(), Token::Fudge);
        assert!(lexer.next_token().is_err());
    }

    #[test]
    fn test_ampersand_token() {
        let mut lexer = Lexer::new("&");
        assert_eq!(lexer.next_token().unwrap(), Token::Ampersand);
        assert_eq!(lexer.next_token().unwrap(), Token::Eof);
    }

    #[test]
    fn test_ability_word_token_case_insensitive() {
        for input in ["Ability", "ability", "ABILITY"] {
            let mut lexer = Lexer::new(input);
            assert_eq!(lexer.next_token().unwrap(), Token::Ability);
            assert_eq!(lexer.next_token().unwrap(), Token::Eof);
        }
    }

    #[test]
    fn test_boost_word_token_case_insensitive() {
        for input in ["Boost", "boost", "BOOST"] {
            let mut lexer = Lexer::new(input);
            assert_eq!(lexer.next_token().unwrap(), Token::Boost);
            assert_eq!(lexer.next_token().unwrap(), Token::Eof);
        }
    }

    #[test]
    fn test_setback_word_token_case_insensitive() {
        for input in ["Setback", "setback", "SETBACK"] {
            let mut lexer = Lexer::new(input);
            assert_eq!(lexer.next_token().unwrap(), Token::Setback);
            assert_eq!(lexer.next_token().unwrap(), Token::Eof);
        }
    }

    #[test]
    fn test_force_word_token_case_insensitive() {
        for input in ["Force", "force", "FORCE"] {
            let mut lexer = Lexer::new(input);
            assert_eq!(lexer.next_token().unwrap(), Token::Force);
            assert_eq!(lexer.next_token().unwrap(), Token::Eof);
        }
    }

    #[test]
    fn test_difficulty_word_token_case_insensitive() {
        for input in ["Difficulty", "DIFFICULTY", "difficulty"] {
            let mut lexer = Lexer::new(input);
            assert_eq!(lexer.next_token().unwrap(), Token::Difficulty);
            assert_eq!(lexer.next_token().unwrap(), Token::Eof);
        }
    }

    #[test]
    fn test_lowercase_difficulty_after_d_separator() {
        // 2ddifficulty: the first 'd' falls back to the separator (next char
        // is 'd', not 'i'), the second lexes the full word.
        let mut lexer = Lexer::new("2ddifficulty");
        assert_eq!(lexer.next_token().unwrap(), Token::Number(2));
        assert_eq!(lexer.next_token().unwrap(), Token::D);
        assert_eq!(lexer.next_token().unwrap(), Token::Difficulty);
        assert_eq!(lexer.next_token().unwrap(), Token::Eof);
    }

    #[test]
    fn test_d_separator_unaffected_by_difficulty_lookahead() {
        // Plain numeric notation still lexes 'd' as the separator.
        let mut lexer = Lexer::new("2d6");
        assert_eq!(lexer.next_token().unwrap(), Token::Number(2));
        assert_eq!(lexer.next_token().unwrap(), Token::D);
        assert_eq!(lexer.next_token().unwrap(), Token::Number(6));
        assert_eq!(lexer.next_token().unwrap(), Token::Eof);
    }

    #[test]
    fn test_digit_d_still_lexes_unchanged() {
        let mut lexer = Lexer::new("D66");
        assert_eq!(lexer.next_token().unwrap(), Token::DigitD);
        assert_eq!(lexer.next_token().unwrap(), Token::Number(66));
        assert_eq!(lexer.next_token().unwrap(), Token::Eof);
    }

    #[test]
    fn test_proficiency_word_token_case_insensitive() {
        for input in ["Proficiency", "proficiency", "PROFICIENCY"] {
            let mut lexer = Lexer::new(input);
            assert_eq!(lexer.next_token().unwrap(), Token::Proficiency);
            assert_eq!(lexer.next_token().unwrap(), Token::Eof);
        }
    }

    #[test]
    fn test_challenge_word_token_case_insensitive() {
        for input in ["Challenge", "challenge", "CHALLENGE"] {
            let mut lexer = Lexer::new(input);
            assert_eq!(lexer.next_token().unwrap(), Token::Challenge);
            assert_eq!(lexer.next_token().unwrap(), Token::Eof);
        }
    }

    #[test]
    fn test_crit_success_and_failure_still_lex_unchanged() {
        let mut lexer = Lexer::new("cs20cf1");
        assert_eq!(lexer.next_token().unwrap(), Token::CritSuccess);
        assert_eq!(lexer.next_token().unwrap(), Token::Number(20));
        assert_eq!(lexer.next_token().unwrap(), Token::CritFail);
        assert_eq!(lexer.next_token().unwrap(), Token::Number(1));
        assert_eq!(lexer.next_token().unwrap(), Token::Eof);
    }

    #[test]
    fn test_marvel_word_still_lexes_unchanged() {
        let mut lexer = Lexer::new("3dmarvel");
        assert_eq!(lexer.next_token().unwrap(), Token::Number(3));
        assert_eq!(lexer.next_token().unwrap(), Token::D);
        assert_eq!(lexer.next_token().unwrap(), Token::Marvel);
        assert_eq!(lexer.next_token().unwrap(), Token::Eof);
    }
}
